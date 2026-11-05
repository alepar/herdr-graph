requirements:
R1 → hg-k4l.1, hg-k4l.12
R2 → hg-k4l.2
R3 → hg-k4l.1, hg-k4l.2, hg-k4l.11, hg-k4l.12
R4 → hg-k4l.2
R5 → hg-k4l.3
R6 → hg-k4l.4, hg-k4l.13
R7 → hg-k4l.5, hg-k4l.6, hg-k4l.7, hg-k4l.13, hg-k4l.15
R8 → hg-k4l.4, hg-k4l.6, hg-k4l.8, hg-k4l.9, hg-k4l.13
R9 → hg-k4l.15
R10 → hg-k4l.7, hg-k4l.14
R11 → hg-k4l.9
R12 → hg-k4l.6, hg-k4l.10
R13 → hg-k4l.6
R14 → hg-k4l.16
R15 → hg-k4l.11
R16 → hg-k4l.2
R-new: the node manager seat cannot be relocated (moved to another tab/workspace) by any plan → (unmapped)
R-new: undoing a seat retirement restores the manager pointers that reparenting rewrote → (unmapped)
R-new: non-seat commands that hit an unavailable daemon tell the agent to retry later and/or get the daemon fixed → (unmapped)

findings (condensed by orchestrator; full text in session transcript):
GAP · B11 node manager cannot be moved (placement)
GAP · R9 undo of retirement vs reparent rewrites
GAP · B7 observed cascades rewrite pointers without a plan
GAP · B10 hydrate/other seat-creating paths
GAP · B10 "seated" vs "created" interpretation
GAP · B2 retry guidance for non-seat commands
UNOWNED-SEAM · manager-pointer rewrite effect kinds
UNOWNED-SEAM · seat.resolve integrity message format
UNOWNED-SEAM · who reply construction hg-k4l.3 vs hg-k4l.16
GAP · B1 hg-k4l.10 read path unstated
GAP · R2 outside-Herdr reported as daemon unavailable
GAP · documentation of manager seats and exit codes
