requirements:
R1 → hg-45e.9
R2 → hg-45e.8
R3 → hg-45e.9
R4 → hg-45e.4, hg-45e.2, hg-45e.3
R5 → hg-45e.5
R6 → hg-45e.13
R7 → hg-45e.10
R8 → hg-45e.15
R9 → hg-45e.10, hg-45e.11
R10 → hg-45e.14, hg-45e.6
R11 → hg-45e.7
R12 → hg-45e.7
R13 → hg-45e.14
R14 → hg-45e.11
R15 → (unmapped)
R-new: every non-apply write path to instruction files goes through the apply gate or is refused → (unmapped)
R-new: approval at apply binds to the exact plan content shown → (unmapped)
R-new: injection fires on startup, resume, clear and compaction in both harnesses → (unmapped)
R-new: harness does not auto-load working-tree instruction files → (unmapped)
R-new: the daemon-derived summary path is outside the closed instruction file set → (unmapped)
Findings (verbatim summary of reviewer A): GAP undo bypass; GAP other write paths audit; GAP exact-change binding; GAP clear/compaction hook coverage; GAP working-tree native loading; UNOWNED-SEAM summary path vs instruction set; UNOWNED-SEAM docs vs command surface; UNOWNED-SEAM affected-seats resolution; GAP R15 tree-level verification. NEEDS-SPEC: hg-45e §2.1/§4.1 and undo routing.
