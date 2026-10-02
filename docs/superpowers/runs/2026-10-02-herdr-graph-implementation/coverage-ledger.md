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
