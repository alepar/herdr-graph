R1 (B4) seat.resolve fails with a distinct integrity error for every reference that does not resolve, including the formerly silent skips; plain read failures stay internal
R2 (B2, B3) herdr-graph seat exits 5 when the daemon is unavailable, telling the agent its context is not loaded, to carry on and to run /seat again later
R3 (B4) herdr-graph seat exits 6 on an integrity error with a not-transient message naming the node manager seat (or the user when none is assigned)
R4 (goal) the seat skill and the SessionStart hook tell the agent what to do when /seat fails, and never to guess its seat
R5 (B1, B14) who, ops, request and undo no longer read the graph without the daemon; they report daemon-unavailable with exit 5
R6 (B5) a seat can point to a manager seat, directly or through its teamspace's default
R7 (B7) manager pointers change only through plans
R8 (B8) the node manager is the root of every seat on the node: it has no manager and cannot be retired
R9 (B6) retiring a seat reparents its reports to the retired seat's own manager
R10 (B10) the first seat created on a node becomes the node manager, and shipped templates create that first seat with the role
R11 (B11) the node manager's tab is reopened rather than retired when it is closed
R12 (B12) a CLI plan command assigns the node manager when none is set, and every graph command warns while it is unassigned
R13 (B13) the same command moves the role; the old node manager and its direct reports become reports of the new one
R14 (B9) who and show display the manager chain
R15 (B5, goal) /seat tells the session who its manager and node manager are
R16 (goal) herdr-graph seat exits 2 when no instance is configured, matching other commands
