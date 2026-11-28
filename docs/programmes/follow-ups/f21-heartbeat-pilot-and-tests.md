+++
type = "task"
mode = "afk"
status = "open"
blocked_by = []
repos = ["amont"]
+++
# The queued heartbeat lacks a live pilot and three tests

## Question

t20 (#547) shipped without three checks: a live pilot that pastes a real queued
hook's line into the plan's Outcome, tests for the arms of
`WayOut::for_snapshot`, and a rehearsal-level test of the heartbeat's stderr
line. Does the heartbeat behave as the plan says on a real queued hook, and
are its arms covered?

## Look at

- The t20 plan's Outcome section, and its pilot line.
- `WayOut::for_snapshot` and its arms, and the tests that already cover them.
- The stderr output of a queued rehearsal run.

## Answer looks like

The plan's Outcome holds a real queued hook's heartbeat line, every arm of
`WayOut::for_snapshot` has a test, and a rehearsal-level test asserts the
stderr line, all in one merged pull request.
