# coverage-amend-design-1 — union of two reviews (A: 18 findings, B: 20), deduped, all verified against the round input

Applied (bead contracts re-synced with the amended spec):
A1  R6 canonical text stale (A F1, B F1) — restate R6 (orchestrator)
A2  hg-blw.4 still listed / hg-blw.10 consumes names .4 (A F15, B F2) — hg-blw.4 is closed+superseded; fix hg-blw.10 text
A3  hg-blw.12 missing status-table tests (B F3): unbound caller writes nothing; no record / corrupt / older schema / no occupant; truncated flag; cut-prefix digest; omitted-part edit stays current; cap-boundary move is stale; record replaced on a new native_session_id; modified/added/removed
A4  Codex hook rewrite unowned (A F2, B F4): codex_hook_command emits --stdin-payload; positive Codex test with session_id; older Codex hook keeps working
A5  Older installed hooks (A F3, B F5.3, B F17): record written with native_session_id:null and stdin_payload:false by a hook run without the flag (A F14 resolved: it writes, with null id); text/JSON say "hook predates --stdin-payload; rerun setup"; doctor flags an outdated owner-marked hook; setup rerun replaces in place (one entry), both harnesses
A6  Bounded stdin read (A F12, B F17): non-TTY only, size- and time-bounded; held-open stdin still prints and exits in budget; hook text byte-identical with and without the flag; session-report gets the same arguments
A7  Per-object error rows (A F5, B F12): hg-blw.1 and hg-blw.2 own and test the error row (dangling template_ref/application), other seats render
A8  hg-blw.2 harness-file shape (A F6, B F6): harnesses list, note, harness_files_note, imports_followed; tests for override shadowing, Git-root bound, Claude AGENTS.md fallback, user-level override
A9  hg-blw.5 log semantics (A F7, B F10): {schema, head, scan, rows}; op|null, kind|null, malformed + one stderr warning; --by AND (incl. rename and retired; requesterless never matches); undone from HEAD; ops pointer on stderr under --json; undo --list unchanged
A10 hg-blw.9 last_change (A F7, B F9): null + last_change_scanned + roster scan object
A11 hg-blw.6 live semantics (A F8, B F7): live_unavailable.{herdr,threads} with exit 0; same_server incarnation match; earlier-incarnation object listed as unmanaged; label|null; unmanaged null when snapshot unavailable; not snapshot().await.ok()?
A12 Reserved envelope fields (A F8, B F8): hg-blw.1 reserves live_unavailable, scan, last_change_scanned
A13 Pinned HEAD CommitId (A F9, B F11): hg-blw.7 owns it; consumers take it; hg-blw.11 asserts one head across components
A14 Scope retired-only rules (A F10, B F16): hg-blw.7 tests retired-only resolve, several-retired ambiguous with ids, instructions show keeps Retired
A15 hg-blw.12 consumes HistoryWalker last-touch (A F11, B F18) + owns the seat --stdin-payload flag and the setup hook-command changes (B F18)
A16 Shared per-clone instructions object (B F13): hg-blw.12 owns the type; hg-blw.2 reserves clones[].instructions of that type; hg-blw.11 reference updated
A17 --json single document, notices to stderr (B F14): hg-blw.7 emit step; hg-blw.11 asserts one document per view
A18 sessions tombstones/rows (A F17, B F15): hg-blw.3 acceptance
A19 docs (A F16, B F19, ledger C11): hg-blw.10 documents Codex/older-hook unknown + rerun setup (and possible Codex re-trust after the command change), live_unavailable exit 0, earlier-server bindings, path last touched, present-not-loaded, --by AND, stderr notices
A20 R14 through the setup-emitted command from a clone with no occupant (A F18, B F20): hg-blw.11

Parked (unattended):
P1  B5/B2 for Codex seats (A F4, B F5): the repo has no evidence Codex's SessionStart payload carries session_id; if it does not, Codex clones read unknown (loaded parts still shown). No human scope change covers this → degraded-verdict "requirement partially unmet — B5 for Codex seats whose hook payload lacks session_id". hg-blw.12 reads session_id when present (A4), so the gap closes by itself if Codex sends it.
P2  Codex hook trust after the command rewrite (A F13): unverified whether Codex keys trust to the command string → documented in hg-blw.10 (A19); parked with P1.
