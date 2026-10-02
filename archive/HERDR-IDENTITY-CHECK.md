# Herdr identity API check

2026-09-27. Read-only inspection; no panes moved, renamed, launched, or closed and no server restarted. Live client/server: Herdr 0.9.1, protocol 22. Matching local source checkout `/tmp/herdr-plugin-v091` is commit `065ef9d6a531c49fb8bee7e818ef837065b21ee9`.

## Confirmed API observations

- `herdr pane current --current` returned this designer's `w8:p1`, tab `w8:t1`, workspace `w8`, terminal ID `term_65c7ec0dbe0f024`, native Codex conversation reference, and revision 29.
- `herdr api snapshot` returns workspaces, tabs, panes, layouts, agents, version and protocol. Pane records contain terminal IDs and public addresses, enabling lookup by terminal identity across the snapshot.
- `herdr pane process-info --current` returned shell PID, foreground process group, and foreground Codex process with `codex --no-daemon`. These are current process observations, not persistent seat identities.
- Installed schema and matching server response types expose no server boot/incarnation identifier. `endpoint_protocol_generation` is a protocol compatibility field, not runtime identity.
- Initial sandbox-denied socket reads failed; approved read-only socket access succeeded. Permission failure must remain unknown/unavailable, never inferred closure.

## Source-qualified behavior

Source links pin the inspected version:

- [Terminal identity allocator](https://github.com/herdrdev/herdr/blob/065ef9d6a531c49fb8bee7e818ef837065b21ee9/src/terminal/id.rs): terminal IDs combine wall-clock microseconds and a process-local counter. Useful runtime discriminators, not a formal globally unique UUID guarantee.
- [Pane API](https://github.com/herdrdev/herdr/blob/065ef9d6a531c49fb8bee7e818ef837065b21ee9/src/app/api/panes.rs): current-pane lookup resolves a supplied caller address; absence falls back to focus. It does not authenticate the calling process. Cross-workspace moves retain the live terminal, change public address, and retain an old-address alias. Rename/layout changes do not allocate a new terminal.
- [Workspace allocation](https://github.com/herdrdev/herdr/blob/065ef9d6a531c49fb8bee7e818ef837065b21ee9/src/workspace.rs): public pane counters increase within a workspace. Workspace counter restoration reserves above surviving workspace IDs; deleted higher workspace addresses can consequently recur after restart.
- [Restore](https://github.com/herdrdev/herdr/blob/065ef9d6a531c49fb8bee7e818ef837065b21ee9/src/persist/restore.rs): public pane numbers can be restored, but terminal IDs are freshly allocated (lines 537 and 630).
- [Persistence schema](https://github.com/herdrdev/herdr/blob/065ef9d6a531c49fb8bee7e818ef837065b21ee9/src/persist/snapshot.rs): pane snapshots retain labels, native session metadata and launch arguments; they do not retain terminal IDs or a graph seat identifier. Runtime metadata tokens are not a demonstrated persistent binding solution.

## Design consequences — proposals

Use public pane IDs as locators and observed terminal IDs as an additional continuity check. A reused public address with a different terminal ID must not auto-adopt the previous graph seat. Verified same-terminal renames can retain the binding; moves require checking team placement. If terminal continuity cannot be established after restore, resolve the intended organizational seat and apply an explicit rebind plan.

Explicit seat identity at launch conveys intent; an activation generation protects graph bookkeeping against stale plans and duplicate activation but does not by itself prove caller identity. Neither `--current`, native session metadata, nor process-info is a complete authenticated current-caller API. Caller verification remains a separate qualification problem if needed for automated binding changes.

These findings support a conservative design with explicit ambiguity handling. They do not prove universally automatic restore recovery or absolute uniqueness across hosts/restarts. Runtime rename/move/restore scenarios were inspected in source, not exercised against this live session.

## Rename observation follow-up

The installed source emits `workspace.renamed` and `tab.renamed` from their API handlers, with IDs and new labels. `pane.updated` exists, but the 0.9.1 `handle_pane_rename` path changes the manual label and marks session state dirty without directly emitting that event. Do not assume every pane rename produces a notification. The event hub retains only 512 in-memory events and drains overflow without a gap error in this version; subscriptions are not durable replay.

Approved design direction: a small non-model process subscribes for prompt updates and periodically reconciles snapshots, persisting observed labels/addresses against verified terminal bindings. Reconcile on connection startup/reconnect as well. This reduces accumulated drift but cannot guarantee capturing a rename immediately followed by a crash/restart, or prove identity from saved labels after restore. The user subsequently chose to propagate Herdr renames to graph names and state paths, preserving durable identity and prior-name history; this does not authorize inferred team reassignment on moves. Current-state lookup and per-seat plugin system threads expose path changes. The user accepts best-effort coverage of ordinary cases; "99.9%" is an aspiration, not a measured guarantee. Implementation is not authorized by this design discussion.
