# Final review inv-3, cluster 3 (docs): docs/verification-matrix.md

Source: base 60e287aadd08af10779df39e625200f28d4b7fa3, head 11ac415754b5edcedf3790ca4aa050e9fbeabba5 (both verified as commits). Slice reviewed: slices/000098.diff, one added section (+23 lines) in docs/verification-matrix.md. Baseline rows B1..B4 are delivered by code in other clusters; this cluster only records the pin and the contract audit, so row tracing leaves the cluster and belongs to the seam reviewer.

## Claims checked against the head tree

- Pin: third_party/herdr-threads gitlink is 7c3c09082e8894ebceec9c060b769b07e359a2ad; the checked-out submodule HEAD matches, and tag v3.6.0 resolves to the annotated tag object 5bcb8a7b... and the commit 7c3c090. Cargo.lock moves herdr-threads 3.1.0 to 3.6.0 and nothing else (diff is one line). Matches the text.
- Wire and schema: PROTOCOL_VERSION is 9 in the pinned wire.rs; node migration 0019_seat_resurrect is the newest (schema 19). The 3.6.0 changelog states no schema change. Matches.
- Audit coverage: threads tags v3.1.1 through v3.6.0 are all rows or grouped rows (3.1.1, 3.1.2, 3.2.0, 3.2.1, 3.2.2, 3.3.0, 3.4.0, 3.4.1, 3.5.0, 3.6.0). Descriptions match the CHANGELOG text. Satisfies hg-lnw.6 acceptance "every minor 3.1.0..3.6.0".
- Service levels: ServiceSessionLevel::required_by puts ResurrectSeat at V4 and FinalizeRetirement at V3, so "keyless FinalizeRetirement ... v3 too" holds at the source level. OBLIGATION_EXPIRY_MS is 24 h, KEYLESS_FINALIZE_MS is 7 d, and both test env names match.
- Adapter mapping (src/threads/adapter.rs restore_outcome, src/ports/threads.rs SeatRestoreRefusal): done, already resolved, transient and definitive splits match the prose exactly, including StoreBusy as busy, OperationPayloadMismatch as InvalidRequest, and a refused v4 registration as Unsupported. The "forget after every definitive refusal" statement matches restore() (forget_completed when the answer is a non-pending Api error). The executor key format matches effects.rs and CURRENT-DESIGN.md.
- Named errors: the upgrade text in the matrix matches adapter.rs level_refused; discovery refuses a 3.5.x daemon through daemon_floor (hg-lnw.25), as stated. GRAPH_SERVICE_LEVEL is V4 and with_level is used at the client build.
- Cross-doc consistency: README lines 221, 223, 239, 309, CURRENT-DESIGN.md 57 and 66 agree with the matrix on pin, floor and error strings.

## Observations

- The pin-time sentence about the fake service daemon is true only at the pin commit. At head src/threads/service_tests.rs models ResurrectSeat (records the request, pops scripted answers, default SeatResurrected) and only the other v3 lifecycle calls answer Unsupported. The paragraph's own next sentence says hg-lnw.7 moved the client, so it reads as history, but "it now answers that operation Unsupported" is wrong for a reader at head.
- The new section is a pin and contract audit. It does not map the resurrect and restart behaviours (B1, B2) to the tiers that check them (tier 3 undo and cold restart, tier 5 real threads). The file's stated purpose is "which contract behaviours are checked against what"; that mapping lives in the README test-tier text and bead evidence, not here.
- Test evidence: the cluster is prose only. Spot checks were read-only; no cargo ran, per the sweep constraint.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- none

## Deferred OK

- docs/verification-matrix.md pin paragraph: "it now answers that operation Unsupported" describes the fake at the pin commit; at head the fake models ResurrectSeat. Reword to "at the pin commit" when the file is next touched.
- docs/verification-matrix.md has no row mapping B1 (keyless undo resurrect) and B2 (frictionless restart) to the tier 3 and tier 5 tests that check them; a follow-up docs bead can add it.
