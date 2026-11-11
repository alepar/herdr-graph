# Final review, cluster 4 (herdr), invocation inv-64d86974-dab5-4fe4-a772-e304978dabd1

Source: base bfeb8f3104dd3783d67a46570c9f272e935012b5, head b9caa7f07c309421854fc87241b76fc40cfda298 (both verified with `git rev-parse --verify <sha>^{commit}`). Caller checkout observed at HEAD b9caa7f, working tree clean; it was not switched or modified.

## Scope of this cluster

One file, `src/herdr/fake.rs`, slice `slices/000064.diff` (+9 lines, one commit: a9dec8a "feat(inspect): roster --live with Herdr agent state, binding match and --unmanaged"). It is a test-double change; no production Herdr client path changes.

`FakeHerdr` gains `snapshots: AtomicUsize`, initialised via `Default::default()` in `FakeHerdr::new`, incremented with `fetch_add(1, SeqCst)` as the first statement of `HerdrApi::snapshot`, and read by `pub fn snapshot_calls(&self) -> usize`.

## Analysis

- Semantics match the doc comment ("faulted ones included"): the increment precedes the `session.snapshot` fault lookup, so every early return (`Unavailable`, `Timeout`/`LostResponse`/`Hang`, `Rejected`) and the passthrough faults (`StartOutcome`, `AppliedLate`) are counted.
- The counter lives outside the `state` mutex. That is fine: it is a monotonic tally read only for assertions, and `SeqCst` on a lone atomic is more ordering than needed but harmless.
- Formatting: the `self.snapshots\n.fetch_add(...)` line break matches rustfmt's default `chain_width` (60); the chain is 65 characters, so rustfmt would break it the same way.
- Consumers at head (`git grep snapshot_calls`): only the `src/daemon/roster_live.rs` unit tests. They assert `== 1` for a successful `roster.live` call, `== 1` for an `Unavailable`-faulted call (so the handler does not retry after a failure), and `== 0` with no Herdr client. Those rigs call the registry handler directly with no background reconciler on the shared fake, so the counts are deterministic.
- Spec fit: design §2.2 ("The daemon takes one Herdr snapshot") and requirement baseline row B8 ("`roster --live` is in scope now"). This cluster provides only the test seam that pins the one-snapshot rule. Tracing B8 from a real production entry point (CLI `roster --live` through the daemon `roster.live` handler to `HerdrApi::snapshot` on the real client) leaves this cluster; that is for the roster-live and seam reviewers. Nothing in this cluster fails to deliver a baseline row.
- `src/herdr/fake.rs` is compiled unconditionally (`pub mod fake` in `src/herdr/mod.rs`, not gated by `cfg(test)` or the `test-support` feature). That predates this branch; the new field adds no meaningful cost.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- none

## Deferred OK
- none
