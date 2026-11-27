R1 Undo of a closed adopted (keyless) tab or pane inside the threads reversible window restores the original herdr-threads seat with its memberships and history (B1).
R2 Undo of a closed keyless seat after 24 h and after 7 d has defined, implemented behaviour, and the undo preview names which applies (B1).
R3 After a Herdr server restart, every graph-managed seat (created or adopted, keyed or keyless, with or without a captured session, shell seats, agents never observed) is back on its own herdr-threads seat with no operator action (B2).
R4 After a restart graph uses the upstream held-seat rebind before relaunching keyless agents, and resumes whenever it has a captured session (B4).
R5 A seat threads still holds after a restart despite graph's rebind is reported in status and doctor with the operator remedy (B2 fallback).
R6 Graph's service session registers service_session_v4 through the upstream PersistentServiceClient level option, and graph pins the herdr-threads release carrying it (B3).
R7 Every herdr-threads minor from 3.1.0 to the pinned release is audited for wire, schema and contract changes graph depends on, and the audit is recorded.
R8 Every pin text site (submodule, Cargo.lock, README, CURRENT-DESIGN.md, CHANGELOG Unreleased, verification matrix, version fixture) names the new pin, with the daemon migration note.
R9 An undo-restored seat whose agent exits while its pane stays open is relaunched (coordinator trial finding 3).
R10 Tier 3 tests cover undo of a closed adopted tab keeping memberships and a Herdr restart; tier 5 tests cover ResurrectSeat against real threads.
R11 Graph-created (keyed) seats keep their existing undo and restart behaviour (no regression).
