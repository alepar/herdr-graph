# Final review — cluster 1 (docs) — epic hg-blw

Invocation inv-4b650ab2-a313-4bcb-9196-17e40854f3ab. Source: base bfeb8f3104dd3783d67a46570c9f272e935012b5, head 6eb6cec786cde8ca070fbf9aed46bb0609ff63ac (both verified with `git rev-parse --verify <sha>^{commit}`). The caller checkout was at 6eb6cec with a clean `git status --short` when the review started; it was not switched or modified. bind-review-source exited 0.

## Scope and sources

Cluster files: the 19 files under `docs/superpowers/runs/2026-11-07-graph-state-inspection/` (slices 000001–000019): the design spec, two design roast reports and the step-back, amend-design-1, the coverage rounds, ledgers and dispositions, friction.md and run.md. All were read at head with `git show`. The acceptance standard is the spec's `## Requirement baseline` (B1–B8, S1). Rows were traced from production entry points at head (`src/cli/mod.rs` command enum, `src/cli/{roster,sessions,log,ops,instructions}.rs`, `src/daemon/compose.rs` registration of `roster_live`). Bead state comes from `bd show`/`bd list --parent hg-blw --all`, and the run ledger from `.superpowers/sdd/hg-blw-plan/progress.md`.

## Baseline trace at head

- B1 (which seats are active): delivered. `herdr-graph roster` is wired at `src/cli/mod.rs:69,92` and is daemon-free. The detailed roster semantics belong to the src-cli and src-inspect reviewers.
- B2 (which files are loaded into context per seat): not delivered. No production path at head shows which files a seat's agent was handed:
  - `instructions stack` does not exist. `src/cli/instructions.rs` has only `InstructionsAction::Show`.
  - The loaded record does not exist: there is no `--stdin-payload`, no `.graph-local/loaded` writer and no LoadedInstructions module (`git grep` over `src` at head finds only doc comments in `src/inspect/output.rs:110-128` and `src/inspect/roster.rs:33`).
  - The roster `instructions` object is always `InstructionsView::unknown()` with `parts: []`, or `error` (`src/inspect/roster.rs:294-295`).
  - The ledger shows hg-blw.2 BLOCKED (toolbox wedged; its code sits on the unmerged branch task-hg-blw.2 at 9bf2777) and hg-blw.12 cancelled. Both beads are still open.
  - No S row covers this.
- B3 (latest modifications: global, space, seat): delivered through `log` (`src/cli/mod.rs:73,94`), `ops --seat/--teamspace/--limit/--json` and roster `last_change` (hg-blw.5, .8 and .9 are closed).
- B4 (list sessions by space/seat/clone, newest first): delivered through `sessions` (`src/cli/mod.rs:71,93`; hg-blw.3 is closed).
- B5 (staleness is shown, not notified): not delivered. No code at head can produce `current` or `stale` (roster.rs:294 only yields unknown or error), so staleness is never shown. The "not notified" half holds trivially. The Codex partial (run.md parked P1: Codex payload id versus occupant id unverified) also has no human scope change row. Once hg-blw.12 lands, it still needs either evidence or an S row.
- B6 (`log` hides bookkeeping by default): delivered in the `log` path (`--all` flag in `src/cli/log.rs`). The detail belongs to the src-inspect reviewer.
- B7 (harness-native files: presence, size and mtime, content never read): not delivered. No command at head lists harness files (no `harness_files` in `src` at head). This is the hg-blw.2 scope, unmerged.
- B8 (`roster --live` in scope now): delivered. `roster --live` goes to the daemon's `roster.live`, registered at `src/daemon/compose.rs:361`.
- S1 (`--unmanaged` only enumerates): the flag is present and `requires = "live"` (`src/cli/roster.rs:30-31`). That the shape enumerates only is for the src-daemon and src-inspect reviewers to confirm.

## Document consistency

- §2.3 (spec line 259) prescribes the Claude hook command `p=$(cat); printf '%s' "$p" | <bin> seat --hook-prompt --stdin-payload; …`. The shell `cat` runs before the binary and has no bound. Bead hg-blw.12 requires "a held-open stdin still prints and exits within the budget", and the spec's Unchanged behavior says "the hook still never fails or blocks its session". Roast round 2 flagged this as an FYI residual ("cannot be met by the emitted Claude command as written"). Neither the spec nor the bead was reconciled afterwards, so hg-blw.12 would be redispatched against a self-contradictory contract.
- The §2.2 roster JSON shape is incomplete against the rest of the spec and the code:
  - It has no `note` field, although §2.3 names `note: "codex session id form differs"` and `src/inspect/output.rs:127-128` carries it.
  - It has no `occupant.transcript`, although §2.5 "Roster addition" adds it.
  - The per-clone `live` value is `null` with no shape for `{agent, live_pane_matches}`, and neither a JSON location for `seat_refusals`/`seat_lost` nor an `unmanaged` placement is shown.
  - The §2.3 record shape has no `stdin_payload`, although hg-blw.12 acceptance requires `stdin_payload false` and a "hook predates --stdin-payload; rerun setup" notice. The roast round 2 FYI asked for this alignment; it was not done.
  - So §2.1's "the JSON is the contract" rule is only partly met by the spec itself.
- The §2.2 text sample shows `retired(reversible)`, but no JSON field carries reversibility. That contradicts the §2.1 typed-field rule. The implementation cannot render it (deferred minor from hg-blw.1).
- The spec header (line 3) still reads "Beads: epic hg-blw (phases hg-blw.1–.6)". It does not record that hg-blw.12 supersedes hg-blw.4, or that hg-blw.7–.12 exist.
- `findings-ledger-design.md` lists the four round-2 entries as `open`. Meanwhile, run.md's parked line says the punch list was "applied inline", and the spec does carry the fixes: Codex re-trust in Honest limits and Unchanged behavior, and `total` plus the footer in §2.5. The ledger status is stale.
- The external citation `https://learn.chatgpt.com/docs/hooks` (spec §2.3) underpins the Codex `session_id` claim. It was not re-verified in this review. The Codex id-form question remains parked (run.md), and the spec keeps the honest `unknown` fallback.

## Cross-cluster notes

- `src/cli/mod.rs:219-225` (`inspect_stubs_not_implemented`) at head still runs `sessions` and asserts "not implemented". `sessions` is implemented (hg-blw.3), so this test is expected to fail at head. The task 4 and task 6 reports describe it failing, and it also reads the real HOME config. The one-line fix exists only on the unmerged task-hg-blw.2 branch (9bf2777). The src-cli reviewer should confirm.
- The root-acceptance sweep (tiers 1–3 in hg-build) is recorded as deferred to the caller, and the ledger reports the hg-build toolbox as wedged. No green full-suite run at head is on record.

## Verdict
not ready (baseline rows B2, B5, B7 undelivered at head: instructions stack and loaded record unmerged, hg-blw.2/.12 open; spec hook-stdin contract self-contradictory)

## Must fix before landing
- B2 undelivered: no production entry point at head shows which instruction files a seat's agent has. `instructions stack` is absent (src/cli/instructions.rs has only Show), there is no loaded-record writer (`--stdin-payload` and .graph-local/loaded are absent), and roster `instructions` is always unknown with `parts: []` (src/inspect/roster.rs:294). hg-blw.2 is BLOCKED (code on unmerged task-hg-blw.2 @9bf2777) and hg-blw.12 is cancelled. No S row covers it.
- B5 undelivered: staleness is never shown at head. No code path yields `current`/`stale` (src/inspect/roster.rs:294-295 yields only unknown/error) because LoadedInstructions (hg-blw.12) is not implemented. The Codex partial (run.md parked P1) also has no human scope change row.
- B7 undelivered: no command at head lists harness-native files with presence, size and mtime (no harness_files in src at head). This is hg-blw.2 scope, unmerged.
- Spec §2.3 line 259 emits a Claude hook `p=$(cat); …` with an unbounded shell read before the binary, which contradicts hg-blw.12's "held-open stdin still prints and exits within the budget" and the spec's Unchanged behavior "the hook never fails or blocks". Reconcile the spec and the bead (bound the shell read or let the binary read stdin once and forward it) before hg-blw.12 is redispatched.
- Epic leaves hg-blw.10 (README and CLI help; README.md is unchanged in base..head and must also carry the A19 Codex re-trust and older-hook caveats) and hg-blw.11 (integration sweep: R12 read-only test, R14 hook→current→stale→log end-to-end) are open with no delivery at head.

## Untested scope
- No test at head asserts the R12 read-only property (HEAD, index and work tree unchanged) across roster, roster --live, sessions, log and ops; that is hg-blw.11's, open.
- No end-to-end test of R14 (hook injection to current to stale to log) exists, because the loaded record is not implemented.
- No tier 1–3 run at head is on record: the root-acceptance sweep is deferred to the caller and the ledger reports the hg-build toolbox wedged.
- src/cli/mod.rs:219-225 inspect_stubs_not_implemented still expects `sessions` to be a stub, so it is expected to fail at head; the fix is only on unmerged task-hg-blw.2 (src-cli reviewer to confirm).

## Deferred OK
- Spec §2.2 JSON shape omits `note`, `occupant.transcript`, the per-clone `live` overlay shape, the `seat_refusals`/`seat_lost` location and `unmanaged` placement; the §2.3 record omits `stdin_payload`, which hg-blw.12 acceptance requires. Sync the spec with the code and bead when hg-blw.12 lands.
- Spec §2.2 text sample `retired(reversible)` has no typed JSON field (violates §2.1); drop it from the sample or add a field.
- The spec header line 3 still says "phases hg-blw.1–.6" and does not record that hg-blw.12 supersedes hg-blw.4.
- findings-ledger-design.md still marks the four round-2 entries `open`, although run.md and the spec show the punch list applied inline.
- The external Codex hooks citation (learn.chatgpt.com/docs/hooks) and the Codex session-id byte-equality remain unverified; they are parked in run.md with an honest `unknown` fallback.
