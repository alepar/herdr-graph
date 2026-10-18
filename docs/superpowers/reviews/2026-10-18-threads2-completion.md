# Threads compatibility and Herdr matrix completion — 2026-10-18

Implementation and review are complete on `threads2-herdr-matrix`, based on main `6c9db8f5d81cf1771d0afd3ea8e1d37a9c8027fa`. The reviewed implementation head is `6c796dba18a9e5dcd78fc278856d615144a25e4c`; this completion record, status updates and the copied rereview add documentation only. Integration remains a separate user decision: no main merge/push, release, install or live restart has occurred.

All four task-scoped spec/quality reviews are approved. The final whole-branch review identified one Important diagnostic false-PASS gap and one Minor stale setup section. One combined fix wave addressed both; [the scoped rereview](2026-10-18-threads2-final-rereview-1.md) reports both ADDRESSED, zero new findings, no deferred/out-of-scope items, and ready to merge. All prior reports remain alongside this record.

Official threads v2.0.4 is pinned at `b83d032a64cb9f665f49806b744f7cc730626819`, protocol9. The final official release check at 2026-10-18T07:34:08Z identified threads2.0.4 and Herdr0.9.5. Exact consumed protocol/service/node/path/discovery APIs are unchanged from2.0.3; the source pin and lockfile package version were updated together.

Final runtime source is `1005d04657a260dc89a55d3b1d37ce6b6f7cdb7e`. On macOS aarch64, each Herdr0.9.1–0.9.5 passed30graph+5API+2actual-threads cases:185 total. All15 cleanup gates and15 diagnostic gates are clean,190 owned roots removed, zero emergency reaping/survivors, and570 inspected raw diagnostic files retained. All-consumer compilation,13 isolation and35 driver tests passed at this freeze. Adapter56/reconnect18 passes are retained at `e621b440` with the same exact dependency and unchanged production source. Full default/no-default/test-support results remain explicitly historical2.0.3 evidence. Commands, failures, exact source heads, checksums and exclusions are in the [qualification handoff](../../releases/2026-10-17-threads2-herdr-qualification.md) and [durable replacement bundle](../../validation/2026-10-18-threads2-log-qualification/README.md).

The original local binary, historical reports/bundle and external checkout were preserved. Source clones, logs and intentionally retained failed roots are outside the disposable plan workspace: `/private/tmp/hg-final-log-qualification-htb_4uwf` and `/private/tmp/hg-task4-qualification-534cd4f_`; failed-matrix35 roots and reproduction-root references remain in the durable reports. Passing-runtime process cleanup succeeded; retained failed files are deliberate review evidence. No retained resources are nested under this plan's scratch workspace.

Fresh clones must initialize submodules; buildable source archives must include the pinned contents. Existing old-node/unbound populated graphs need the documented reviewed reconnect. Old plans require fresh review; ordinary undo cannot cross the namespace boundary. Native agent/model inference, actual remote continuity/restore holds, Linux/Intel runtime and live session handoff remain excluded.

## Rulings preserved verbatim

Ruling: Continue on a feature branch in the coordinated shared checkout — previous session established shared-checkout ownership/artifact preservation and user now authorizes new implementation — recover through commits/worktree if concurrent ownership appears.

Ruling: Keep graph service channels local and qualify Herdr hosts explicitly — threads2 service channels reject remote seats and local pane IDs are not globally unique — distributed graph orchestration requires a later design.

Ruling: Initial Herdr sweep uses official0.9.1 through0.9.5 with upstream drift detection — matches the requested threads sweep and existing support floor — newly released versions need explicit checksum-list refresh and qualification.

Ruling: Resolve graph-managed pane seats through public host-qualified ResolveSeat rather than ambiguous Seats queries — node2 shares multiple Herdr hosts, guarded resolve proves exact host/target and respects restore holds — graph may allocate a bare threads seat before agent enrollment, but never creates bindings, checks in, accepts or ACKs.

Ruling: Reconnect commits an explicit node/epoch marker and preserved migration snapshot before updating the local binding, with per-epoch service journals — Git is the recoverable transition authority and old intents must remain tied to their original node — reconnect is not ordinary undo; reversing it needs another reviewed reconnect, and old external participation must be re-established explicitly.

Ruling: Require fresh plan review after reconnect even for otherwise equivalent old plans — old effect equivalence cannot prove namespace intent — users must re-plan pending pre-boundary changes, including unrelated ones.

Ruling: Pin herdr-threads as a Git submodule at the verified official release instead of following an absolute external symlink — the external checkout moved to unreleased code during qualification — fresh clones must initialize submodules and source archives must include the pinned dependency contents.

Ruling: Adopt verified official threads v2.0.4 at b83d032a64cb9f665f49806b744f7cc730626819 for replacement qualification — it became latest stable during the run and exact public service/protocol/node API comparison is unchanged — source pin/lock and actual-daemon evidence must move together; v2.0.3 feature results remain explicitly historical, and later releases need renewed qualification.
