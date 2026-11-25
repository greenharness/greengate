+++
type = "task"
mode = "afk"
status = "open"
blocked_by = []
repos = ["amont", "relais"]
+++
# A clock seam for `Stall`'s admitted-at term

## Question

`greenharness-slots` 5.0.0 already ships the injected-clock seam:
`Stall::observe_at(queue, elapsed, now_ms, admitted_ms)` takes every time it
needs. Only the convenience `Stall::observe` reads the real clocks and calls
the private `admitted_at(&waiter.dir)`, and `Stall::new` reads
`Instant::now()`. relais's `take()` calls `observe`, so its
`a_stalled_line_ends_the_wait_and_the_row_says_so` test sleeps 20 ms per poll
through a 1 s real window (marked `holds-until` in
`relais/crates/relais/src/verify/heavy/tests.rs`). Can relais's `take()` call
`observe_at` with values from its own clock, and does it then need the real
admission stamp, which would take a public `admitted_at(dir)` returning an
`Option`, not a bare `u128`? relais must not parse the `admitted-at` file
itself: the slots crate owns that format (ADR-0075). No published signature
changes.

## Look at

- `crates/greenharness-slots/src/lib.rs`: `Stall`, `observe`, `observe_at`,
  `admitted_at`.
- relais ADR-0015 and the plan
  `docs/plans/2026-11-23-admission-counts-holders-stalled-waits.md`, whose
  Decision log keeps relais's own `take()` loop and records the real sleep.

## Done when

relais's `take()` drives `Stall` from its own clock (with a public
`admitted_at` added in a minor crate release if it needs the stamp) and drops
the sleep and its `holds-until` note, or the ticket records why the real-clock
test stays.

Filed from the amont inbox (2026-11-24, session 9abffa07) at the t11 close.
