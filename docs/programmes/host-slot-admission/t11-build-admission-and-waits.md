+++
type = "task"
mode = "afk"
status = "open"
blocked_by = ["t09-journal-holders-and-hold-time", "t03-unqueued-fallback"]
repos = ["amont", "relais"]
+++
# Build: admission counts holders, waits end on a stalled line

## Question

Build what t01, t02 and t03 decided, as one plan in amont (the slot crate and
greengate) with relais's matching change. The load test becomes
`load1 < cores + 4·Σjobs`. A wait ends after 300 s with no progress, under the
existing ceiling. When a wait ends, the check does what t03 chose.

## Look at

- The Resolutions of t01, t02 and t03, and their test seams.
- ADR-0009 and ADR-0013 through aval: the plan says which one it amends.

## Answer looks like

A merged plan and release of greengate and relais, installed on this machine,
with the red tests from t01 and t02 green. Then [t08](t08-workday-measure.md)
can start.
