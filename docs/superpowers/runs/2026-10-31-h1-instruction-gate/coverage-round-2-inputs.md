## Goals

### hg-45e (root)
No agent can change what another agent is instructed to do without the user approving that exact change in their harness. Every instruction source is editable only through a plan committed by `herdr-graph apply`, the summarizer writes summaries through a dedicated atomic API, and every bound seat receives its full instruction stack inline from the committed graph revision at session start, clear and compaction.

## Task tree

- hg-45e · H1 instruction gate, summary API, inline instructions (epic) · Spec: docs/superpowers/runs/2026-10-31-h1-instruction-gate/2026-10-31-h1-instruction-gate-design.md (branch super-auto/h1-instruction-gate). · deps: none
  - hg-45e.1 · Seam contract: GraphId (IdKind::Graph) · Spec §2.3. · deps: none
      owns: GraphId. Files: src/model/ids.rs, src/model/graph.rs, src/store/layout.rs.
  - hg-45e.2 · LineDiff and plan diff rendering · Spec §2.4. · deps: none
      owns: LineDiff. Files: src/plan/line_diff.rs, src/plan/mod.rs, src/plan/commands.rs, src/plan/types.rs (describe).
  - hg-45e.3 · InstructionSource: instruction scopes, files and paths · Spec §2.1 and Key abstractions. · deps: hg-45e.1
      owns: InstructionSource.
      consumes: GraphId. Files: src/store/instruction_source.rs, src/store/mod.rs, src/store/layout.rs.
  - hg-45e.4 · InstructionsKind: plan instructions set|clear · Spec §2.2, §2.4. · deps: hg-45e.2, hg-45e.3
      owns: InstructionsKind.
      consumes: InstructionSource, LineDiff. Files: src/plan/instructions.rs, src/plan/mod.rs, src/model/change.rs, src/model/action.rs, src/daemon/compose.rs, src/plan/tests.rs.
      owns: InstructionsEdit action record schema (compensation {file, before, after}; consumed by hg-45e.5).
  - hg-45e.5 · Undo for instruction edits · Spec §2.5. · deps: hg-45e.4
      consumes: InstructionsKind. Files: src/undo/compensate.rs, src/undo/candidates.rs, src/undo/preview.rs, src/undo/mod.rs, src/undo/tests.rs.
      consumes: InstructionsEdit action record schema.
  - hg-45e.6 · InstructionRenderer and TemplateParticipation · Spec §4.1. · deps: hg-45e.3
      owns: InstructionRenderer, TemplateParticipation.
      consumes: InstructionSource. Files: src/bootstrap/instructions.rs (new), src/bootstrap/mod.rs (bound_paths extraction only), src/bootstrap/tests.rs.
  - hg-45e.7 · SessionStart hook injects instructions; seat --instructions · Spec §4.3, §4.4. · deps: hg-45e.6
      consumes: InstructionRenderer. Files: src/cli/seat.rs, src/bootstrap/mod.rs (wants_seat_prompt helper if shared), src/bootstrap/tests.rs or tests/.
      owns: SessionStart header text (spec §4.4).
  - hg-45e.8 · Summary API: request complete --summary-from · Spec §1.1-1.4. · deps: none
      owns: (extends RequestComplete mutation). Files: src/model/b64.rs (new), src/model/mod.rs, src/bootstrap/content.rs (import only), src/cli/request.rs, src/transcripts/mod.rs, src/transcripts/mutations.rs, src/transcripts/tests.rs.
  - hg-45e.9 · Remove content write · Spec §1.5. · deps: hg-45e.8
      consumes: request complete --summary-from (summary API). Files: as listed.
  - hg-45e.10 · ApprovalGate: harness ask rule on apply in setup claude/codex · Spec §3.1-3.3. · deps: none
      owns: ApprovalGate. Files: src/bootstrap/setup_claude.rs, src/cli/setup.rs, src/cli/plan.rs, src/bootstrap/tests.rs (setup tests).
  - hg-45e.11 · Docs and graph skill for summaries, instruction plans and the apply gate · Spec §3.3, §3.5, §5. · deps: hg-45e.10, hg-45e.13, hg-45e.14, hg-45e.15, hg-45e.4, hg-45e.7, hg-45e.8, hg-45e.9
      consumes: CLI surface of hg-45e.4, .7, .8, .9, .10, .13, .14, .15.
  - hg-45e.12 · Configuration smoke: three instruction scopes and two harness gates · Spec §6. · deps: hg-45e.10, hg-45e.15, hg-45e.4, hg-45e.7
      consumes: InstructionsKind, seat --instructions, ApprovalGate. Files: tests/instruction_smoke.rs (new), tests/support/.
  - hg-45e.13 · instructions show: read-only instruction CLI · Spec §2.2. · deps: hg-45e.3
      consumes: InstructionSource. Files: src/cli/instructions.rs (new), src/cli/mod.rs.
  - hg-45e.14 · seat --json instructions/data contract and seat skill · Spec §4.2, §4.5. · deps: hg-45e.3, hg-45e.6
      consumes: InstructionRenderer, InstructionSource. Files: src/bootstrap/mod.rs (seat_reply, bound_paths output keys, render_seat), src/bootstrap/tests.rs, skills/seat/SKILL.md.
      owns: seat --json instructions/paths contract (instructions, paths.data, paths.clone_record).
  - hg-45e.15 · doctor harness_gate check · Spec §3.4. · deps: hg-45e.10
      consumes: ApprovalGate. Files: src/daemon/doctor.rs, its tests (isolated config dirs).
