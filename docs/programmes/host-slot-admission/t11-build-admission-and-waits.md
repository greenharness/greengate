+++
type = "task"
mode = "afk"
status = "closed"
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

## Resolution

Built and installed. greengate 5.0.0 (#516, release #519) carries t01's
admission rule, t02's stall end and t03's deferral; relais v0.83.0 (#573,
release #580) moved to `greenharness-slots = "5"`; greenbelay 6.15.0 (#331)
advises on `slot run --now`. t01's admission test fails on the old main and
passes from 5.0. t02's red test (a waiter at position 10 with one admission
every 60 s does not time out after 400 s, and its pair, no admission for the
window, stalls) is kept in the crate as
`a_wait_in_a_moving_line_never_stalls` and
`a_line_with_no_progress_for_the_window_stalls`
(`crates/greenharness-slots/src/lib.rs`), green from 5.0.0 and still on 5.1.1.
The stall end it tests now applies to `slot run` and relais; a hook check
waits to its ceiling (t18). The post-install count ([t17](t17-count-after-5-0-install.md))
fired the 2× trigger on hook deferrals; the person chose the narrow fix
[t18](t18-plan-hook-stall-narrow-fix.md), shipped as greengate 5.1.1 (#526,
release #527), installed 2026-11-26 16:24 CET: a hook check waits to its
ceiling and no longer ends on a stall. The plan's remaining measure is
[t19](t19-count-after-5-1-1-install.md), the first half of t08.
