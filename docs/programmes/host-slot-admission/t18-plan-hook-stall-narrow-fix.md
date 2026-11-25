+++
type = "task"
mode = "hitl"
status = "open"
blocked_by = []
repos = ["amont"]
+++
# Plan greengate 5.0.1: no stalled-line deferral on the hook path

## Question

[t17](t17-count-after-5-0-install.md) found 141.8 hook deferrals per 12 h
after the 5.0 install, against a baseline of 31.8 (4.5×, past the plan's 2×
rollback rule). The person chose a narrow fix over a revert. Which change to
the hook path's wait (`wait_for_slot` in
`crates/greengate-runtime/src/host_slots.rs`, from #516) stops the 75
`"end":"stalled"` deferrals while keeping `slot run`'s stall end, `--now`,
`holders`, and relais on crate 5?

## Look at

- t17's `## Interim count`: deferrals by end, name and session.
- The plan `docs/plans/2026-11-23-admission-counts-holders-stalled-waits.md`,
  the hook rows of its wait table, and ADR-0022.
- The 83 `"end":"ceiling"` hook deferrals. Before 5.0 they ran unqueued at
  the ceiling. State whether 5.0.1 keeps them deferred or brings back
  unqueued runs, against `heavy-build-unqueued` (greenbelay 6.15).

## Answer looks like

A plan, reviewed by the panel, that amends ADR-0022 for the hook path and is
released as greengate 5.0.1 through `tag-release`. Its Verification repeats
t17's count 24 h after the 5.0.1 install, against the 31.8-per-12 h
baseline, and that count is the first half of t08.
