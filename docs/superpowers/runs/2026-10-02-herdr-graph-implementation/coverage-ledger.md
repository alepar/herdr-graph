c1 · r1 · GAP · R23 · applied — hg-zmi.5 owns test-isolation guard (live socket/threads/memory observer refusal)
c2 · r1 · GAP · R6 · applied — hg-zmi.17 owns op supersession (+test)
c3 · r1 · GAP · R26 · applied — hg-zmi.18 registers reminder loop; edge .18←.17
c4 · r1 · GAP · R26 · applied — hg-zmi.18 registers all kinds/CLI; edges .18←.9,.10,.13,.17
c5 · r1 · GAP · R22 · applied — hg-zmi.19 flows extended; edges .19←.13,.17
c6 · r1 · GAP · R20 · applied — hg-zmi.13 consumes pending invitations/ops queries; edges .13←.11,.17
c7 · r1 · GAP · R22 · applied — hg-zmi.19 matrix includes .15/.16 results; edges .19←.15,.16
c8 · r1 · GAP · R9 · applied — resurrect in hg-zmi.6 owns, rebind in hg-zmi.17 owns
c9 · r1 · GAP · R25 · applied — profiles table in hg-zmi.1 seam; applied by hg-zmi.7
c10 · r1 · GAP · R14 · applied — hg-zmi.9 owns template copy
c11 · r1 · GAP · R28 · applied — hg-zmi.6 emits act_ for retire/resurrect
c12 · r1 · GAP · R2 · applied — hg-zmi.6 rename kind writes name_history
c13 · r1 · GAP · R27 · applied — hg-zmi.11 owns opt-in real threads test
c14 · r1 · UNOWNED-SEAM · occupant relaunch hook · applied — hg-zmi.7 owns
c15 · r1 · UNOWNED-SEAM · effect executor registration · applied — hg-zmi.7 owns; hg-zmi.11 registers
c16 · r1 · UNOWNED-SEAM · effect record schema · applied — adopted existing seam contract hg-zmi.1 (owns schema)
c17 · r1 · UNOWNED-SEAM · template semantics · applied — edge .13←.9, acceptance hydrates via engine
c18 · r1 · UNOWNED-SEAM · launch env contract · applied — adopted seam contract hg-zmi.1
c19 · r1 · UNOWNED-SEAM · relay confirmation surface · applied — hg-zmi.6 owns
c20 · r1 · UNOWNED-SEAM · action-record envelope · applied — adopted seam contract hg-zmi.1
c21 · r1 · UNOWNED-SEAM · summaries key + role marker · applied — adopted seam contract hg-zmi.1; resolver defaults in hg-zmi.6
c22 · r1 · UNOWNED-SEAM · transcript/ns byte ranges · applied — adopted seam contract hg-zmi.1
c23 · r1 · UNOWNED-SEAM · delivery capability · applied — query in hg-zmi.1 trait, detection owned by hg-zmi.11
c24 · r1 · UNOWNED-SEAM · pending ops/invitations query · applied — owned by hg-zmi.17 / hg-zmi.11, consumed by hg-zmi.13
Seam integration for c16/c18/c20-c23: covered by hg-zmi.19 tier-3 flows and the root integration sweep (no separate Seam integration beads; existing seam contract adopted).
c25 · r2 · GAP · R22 matrix ownership · applied — hg-zmi.14 placeholder, hg-zmi.19 owns README matrix content
c26 · r2 · UNOWNED-SEAM · manifest ↔ skills/hook · applied — no manifest entries exist for Claude skills; hg-zmi.13 setup claude installs skills+hook, README documents
c27 · r2 · GAP · plugin loads in private Herdr · applied — hg-zmi.19 flow + edge .19←.14
c28 · r2 · UNOWNED-SEAM · transcript watcher loop · applied — hg-zmi.12 owns
c29 · r2 · UNOWNED-SEAM · bookkeeping kinds · applied — category flag in hg-zmi.6; kinds owned by hg-zmi.7 (effect/binding) and hg-zmi.12 (tr_/rq_)
c30 · r2 · GAP · R28 undo consumes plan act_ · applied — hg-zmi.10 consumes + acceptance
c31 · r2 · GAP · R5 crash-injection · partially applied — hg-zmi.3 already had failpoint crash acceptance (full description); added harness to owns line and mid-effect crash acceptance to hg-zmi.18
c32 · r2 · GAP · R19 summarizer template named · rejected — hg-zmi.13 full description already ships system-summarizer (owns line now names it too)
c33 · r2 · GAP · R1 herdr-threads dependency · rejected — hg-zmi.1 full description adds the path dependency via third_party symlink
c34 · r2 · GAP · R13 e2e template/cascade/undo flows · partially applied — hg-zmi.19 full description already had template edit, cascades, undo, app retire; added undo caller-pane adoption flow
c35 · r2 · GAP · R22 real-agent credentials · applied — hg-zmi.5 credential pass-through rule
c36 · r2 · UNOWNED-SEAM · teamspace retire/resurrect placement · applied — all retire/resurrect in hg-zmi.6; hg-zmi.17 teamspace rename only
c37 · r2 · UNOWNED-SEAM · participation intent → threads · applied — edge .11←.17, consumes
c38 · r2 · UNOWNED-SEAM · session-id capture rule · applied — hg-zmi.8 consumes + per-harness tests
c39 · r2 · UNOWNED-SEAM · guard for real threads test · applied — edge .11←.5, consumes
c40 · r2 · UNOWNED-SEAM · superseded op state · applied — hg-zmi.3 owns states + supersedes link
sweep · root integration sweep hg-zmi.20 created, depends on hg-zmi.1–.19
g1 · graph · GRAPH-EDGE · hg-zmi.19 <- hg-zmi.16 · parked — drop (safe no): only tier-4 matrix rows consume .16 results; would cut depth 11→10
g2 · graph · GRAPH-EDGE · 18 other critical edges · kept — each carries a real artifact
g3 · graph · PROPOSAL · hg-zmi.7/.8/.11/.12/.13 · parked — seam-contract observer/reconciler boundary (binding write-back kinds) could cut depth to 9
