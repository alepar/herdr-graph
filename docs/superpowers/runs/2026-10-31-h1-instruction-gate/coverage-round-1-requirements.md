R1 herdr-graph content write exits 2 with guidance; no content.write daemon command or content_write mutation exists; legacy ContentWrite records still deserialize
R2 request complete --summary-from writes the summary at the daemon-derived path in the source seat's folder (live or archived) atomically with completion; identical retry no-op, conflicts rejected
R3 the shipped summarizer template and all tests use the summary API instead of content write
R4 plan instructions set|clear exists for global, teamspace and seat with the closed file set, prints a unified diff and affected seats, and yields stale_plan when the file changed after planning
R5 applied instruction edits are undoable, with a conflict when the file drifted
R6 instructions show prints committed instruction files without a daemon
R7 setup claude and setup codex install an ask rule on herdr-graph apply idempotently and uninstall only what they added
R8 doctor warns when a harness with the owned hook lacks the apply ask rule
R9 the relay hint and graph skill tell agents to show the plan and let the harness ask the user on apply
R10 seat --json carries the instruction stack rendered inline from the committed revision in the fixed order with labeled sources, and lists other folder files only as data
R11 seat --instructions prints the render without a daemon
R12 the SessionStart hook injects the header plus render in bound panes (Claude and Codex), truncates explicitly, and never fails
R13 the seat skill follows instructions.text and treats data as non-instructions
R14 CURRENT-DESIGN, GRAPH_STATE, CREATING_NEW_GRAPH, README and CHANGELOG document the new commands and the removal
R15 cargo test, cargo test --features test-support and cargo check --locked --offline --all-targets --all-features pass
R16 no write path to instruction files remains other than a plan committed by herdr-graph apply
R17 the approved apply is bound to the exact plan content shown (plan hash)
R18 instruction injection fires on startup, resume, clear and compaction (Claude; Codex verified or documented)
R19 instructions never come from the working tree or harness-native loading of graph folders
R20 the derived summary path is disjoint from the instruction file set
