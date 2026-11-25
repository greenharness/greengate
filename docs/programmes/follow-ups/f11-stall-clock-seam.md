+++
type = "task"
mode = "afk"
status = "open"
blocked_by = []
repos = ["amont", "relais"]
+++
# A clock seam for `Stall`'s admitted-at term

## Question

`greenharness-slots` 5.0.0's `Stall` reads the last admission from the slot
directory with the real clock, and `admitted_at` is private, so relais's
`a_stalled_line_ends_the_wait_and_the_row_says_so` test sleeps 20 ms per poll
through a 1 s real window (marked `holds-until` in
`relais/crates/relais/src/verify/heavy/tests.rs`). Should the crate expose a
public `admitted_at(dir)`, or take the admitted-at time as an argument of
`Stall::observe_at`, so that test drives the window from its `ManualClock`?

## Look at

- `crates/greenharness-slots/src/lib.rs`: `Stall`, `observe`, `observe_at`,
  `admitted_at`.
- relais ADR-0015 and the plan
  `docs/plans/2026-11-23-admission-counts-holders-stalled-waits.md`, whose
  Decision log keeps relais's own `take()` loop and records the real sleep.

## Done when

The crate takes the seam in a minor release, relais drops the sleep and its
`holds-until` note, or the ticket records why the real-clock test stays.

Filed from the amont inbox (2026-11-24, session 9abffa07) at the t11 close.
