# Final review inv-3, cluster 31 (src-undo)

Source: base 60e287aadd08af10779df39e625200f28d4b7fa3, head 11ac415754b5edcedf3790ca4aa050e9fbeabba5 (both verified with `git rev-parse --verify`). The caller checkout was at 11ac415 with only `?? evidence/sweep/` untracked, observed at start. No cargo was run: the integration worktree hosts a background sweep, and this cluster's tests are cheap unit tests whose passing runs are recorded in evidence logs (for example evidence/hg-lnw.14/final.log). Inputs: slices 000198 (src/undo/preview.rs) and 000199 (src/undo/tests.rs), digest.md, and the design doc docs/superpowers/runs/2026-11-26-resurrect-seat-frictionless-restart/2026-11-26-resurrect-seat-frictionless-restart-design.md (§3, §6).

## What the cluster does

`seat_window_note(c, now)` builds one warning per retired seated clone, naming the §3 window for its threads seat. A keyed seat, or a clone with no seat, gets no note. A keyless seat whose committed fact reads `Retired{reversible: true}` gets the under-24 h, 24 h to 7 d or final wording, based on `retired_at` (taken from the fact, falling back to `c.retired.at`). `Retired{reversible: false}` always reads final. A keyless clone with no fact, or whose fact names another seat, gets the same windowed wording, conditioned with "if threads reports the close reversible". Any other fact state (`Resolved`, `Unresolved`, `KeyConflict`) returns None. `restore_preview` calls it at both `clone.resurrect` sites: clones of a restored seat, and standalone restored clones. The second loop skips clones whose seat is restored, so each clone is noted once. The thresholds are `KEYLESS_OBLIGATIONS_SETTLE_AFTER` (24 h) and `KEYLESS_REVERSIBLE_FOR` (7 d) from src/ports/threads.rs:190-191. The wording matches §3: under 24 h everything returns; from 24 h to 7 d receipts and ordinary invitations have settled and graph re-issues its seat-wide invitations; after 7 d the seat is final, with a fresh seat, re-invited graph channels and lost external memberships.

## Baseline trace

The design has a `## Requirement baseline` (B1 to B4, no human scope changes). This cluster delivers none of B1 to B4 by itself. B1, resurrecting the seat on undo, is executed by src/threads/effects.rs and the ThreadsExecutor, outside this cluster: that path is the seam reviewer's to trace. The preview note is a §3 spec-body requirement ("The undo plan (`herdr-graph undo` preview) names which of the three applies for each seat, so the warning the trial found missing exists"), and this review judges it on those terms.

## Stale pre-close fact analysis

Graph writes seat facts only for Active clones that have a desired pane (`seated_clones`, src/observe/seat_observation.rs:62-74; written through `append_seat_ops`, src/observe/step.rs:792-803). Once a clone is retired, its `seat_fact` is frozen at the last read before retirement. Only src/threads/assignment.rs:76 clears it. Graph classifies a closed tab from the Herdr snapshot, independent of threads (src/observe/classify.rs:341-358, `CascadeRule::Tab`, mechanism `ObservedTabClose`). So when graph sees the tab gone before threads reports the seat retired, the clone retires holding `Resolved{old pane}` (or `Unresolved`). Graph-initiated closes behave the same way. The tier-3 helper `close_and_wait_reversible` (tests/adopt_private_herdr.rs:927-950) waits for graph to retire the clone first, and only then for threads to retire the seat reversibly: this ordering is the expected one, not an edge case. In that state `seat_window_note` reaches `Some(_) => return None`, and the undo preview is silent for exactly the keyless tab-close undo that §3 targets. An unread fact (`None`) gets the conditional note, so the silence comes from the fact-state match, not from missing data. Execution is unaffected: after undo the clone is Active again and the fact is re-read before any want is planned. No test covers a retired keyless clone holding a pre-close `Resolved` fact. `restore_preview_names_the_keyless_seat_window` patches a `Retired` fact in after the cascade, so it never exercises the production ordering. The inv-2 review of this code (final-review-inv-2.cluster-32.md) rated this a degraded case. The classify path above shows it is the ordinary `ObservedTabClose` path whenever graph observes the close first.

## Other checks

- Clock skew (threads' `retired_at` ahead of graph's `now`) gives a negative age, which falls into the under-24 h branch. That is acceptable.
- A `reversible: true` fact older than 7 d is described as final, consistent with threads finalizing at 7 d. The action itself follows the fact re-read at execution.
- A legacy fact without `keyed` deserializes as keyed:true at the field level. The `CloneRecord` deserializer overrides that for a keyless clone (cluster 25), so a legacy keyless clone is not silenced by the `keyed: true` arm.
- The test helper extension to `Patch` (`keyless`, `fact`) is test-only and uses the record's own `threads_seat`.

## Verdict
not ready (undo preview is silent for a keyless clone retired by an observed tab close before threads retired its seat, the ordinary path)

## Must fix before landing
- src/undo/preview.rs `seat_window_note`: a retired keyless clone whose committed `seat_fact` is a pre-close read (`Resolved`/`Unresolved`/`KeyConflict`) returns None via `Some(_) => return None`. Facts are frozen at retirement (seated_clones only covers Active clones), and `ObservedTabClose` cascades are classified from the Herdr snapshot before threads reports the seat retired (tests/adopt_private_herdr.rs:927-950 waits in that order). So §3's "undo preview names which of the three applies for each seat" fails on the ordinary keyless tab-close undo. Treat a non-`Retired` fact on a retired clone like an unread fact: give the conditional windowed note from `c.retired.at`, gated on `c.keyless`. Add a unit test with a `Resolved` fact on a retired keyless clone.

## Untested scope
- `seat_window_note` with a pre-close `Resolved`/`Unresolved`/`KeyConflict` fact on a retired keyless clone (the production ordering for an observed tab close).
- `restore_preview`'s second `clone.resurrect` site (standalone restored clone whose seat is not restored, for example undo of a pane-close cascade) has no window-note test; only the restored-seat site is exercised.
- A fact naming a different seat than `threads_seat` (the `filter(|f| &f.seat == seat)` fallback to the unread wording) is not tested.
- No preview-level test checks the under-24 h or final wording through `herdr-graph undo`; only the 24 h to 7 d wording is asserted end to end through `plan`.

## Deferred OK
- A keyed seat whose fact reads `Retired{reversible: false}` (keyed final, succession to a fresh seat) gets no membership-loss note; that is pre-existing behaviour, and §3's windows are defined for keyless seats.
- Thresholds are graph-side constants mirroring threads' windows; if threads changes them, the preview wording drifts (no threads clock fields exist, per §3).
