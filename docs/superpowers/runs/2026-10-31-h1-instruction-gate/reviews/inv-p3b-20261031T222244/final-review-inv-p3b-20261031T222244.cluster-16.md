# Final review — cluster 16 (templates), epic hg-45e

Source: base c77d9ad166dd2ada5b5a5768b499d12da4c36987, head bce6589afead4c031ac9ecfa3a094c9a5c4be161 (both verified as commits). Caller checkout observed at HEAD bce6589 with a clean status before and during the review; nothing in the caller tree was modified apart from this artifact and the authorized ledger binding.

## Scope

One file: `templates/system-summarizer/members/summarizer/AGENTS.md` (slice 000085). The change drops the `herdr-graph content write --object <source seat id> --rel summaries/... --from <tmp file>` step and replaces `request complete <rq> --output summaries/<tr>-<start>-<end>.md --covered <start>-<end>` with `request complete <rq> --summary-from <tmp file> --covered <start>-<end>`, plus a sentence saying the daemon stores the summary at `summaries/<tr>-<start>-<end>.md` in the source seat's folder and completes the request in one commit.

## Against the spec

- Design §1.5 requires updating this shipped template so it no longer calls `content write`. Met: at head the file has no `content write` reference and no `--output`.
- §1.1: `--summary-from` is mutually exclusive with `--output`. The CLI at head (`src/cli/request.rs`) declares `output` with `required_unless_present = "summary_from"` and `conflicts_with = "summary_from"`, so the documented command parses. A unit test covers this (`complete_accepts_output_or_summary_from_not_both`).
- §1.2: the path is derived from the covered range. `src/transcripts/mutations.rs` `write_summary` builds `summaries/{tr.id}-{covered.start}-{covered.end}.md` under the located seat folder. That matches the template's description, because the `<start>-<end>` placeholders in the path sentence are the same ones passed to `--covered`.
- §2.2a content clause: the template is rendered inline (§4.1 step 3, member `AGENTS.md`), so it has to be admissible. The file at head is pure printable ASCII plus newlines (a grep for anything outside 0x0A/0x20-0x7E finds nothing), so it renders and is not replaced by a placeholder.
- Acceptance 7 ("summarizer template reflects the new commands"): met. `src/bootstrap/tests.rs` (~line 1500) applies `system-summarizer` and checks that the hydrated member `AGENTS.md` contains the new `request complete ... --summary-from <tmp file> --covered <start>-<end>` line, the `summaries/<tr>-<start>-<end>.md` path, the ack line and the leftover-scan lines. It also checks that no seat `AGENTS.md` copy is seeded (§1.5 hydration change). `tests/integration_sweep.rs` drives `--summary-from` end to end.
- The flow is still correct: the subagent writes a temp file, and the summarizer completes only after the file exists. A failed subagent still leaves the request open for the liveness scan. The stored path no longer depends on the agent supplying it, which is the point of §1.

## Observations

- In step 2, `[start, end)` means the requested range. In step 3, the same placeholder names mean the covered range, which may be smaller. The path sentence is technically right (the daemon uses the covered range), but a reader could take it to mean the requested range. This is cosmetic and changes no behaviour.
- `<tr>` is never defined in the template (the request carries the transcript id). This was already true before the change and is harmless, since the agent no longer builds the path.
- Existing instances keep their old copied template (by design, §1.5 migration note in CHANGELOG). That migration belongs to another cluster.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- none

## Deferred OK
- Template reuses `<start>-<end>` for both the requested range (step 2) and the covered range (step 3 path sentence); consider "covered range" wording in the path sentence for clarity.
- `<tr>` placeholder is undefined in the template text (pre-existing; no longer load-bearing since the daemon derives the path).
