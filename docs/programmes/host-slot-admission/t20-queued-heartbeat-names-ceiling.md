+++
type = "task"
mode = "afk"
status = "open"
blocked_by = []
repos = ["amont"]
+++
# The queued heartbeat names the ceiling and the way out

## Question

A hook check can now wait up to `greengate.timeout` (3600 s) in a line that
does not move. The live heartbeat reads `queued 1m00s for a host slot
(greengate.hostSlots 2)` and names neither when the wait ends nor
`GREENGATE_ON_BUSY=now`. Should it say, within 80 columns, where it ends and
how to skip it? Carried from the t18 plan review (tui).

## Look at

- `crates/greengate-runtime/src/live.rs` near the queued heartbeat (about line
  1296; its test already checks width) and `host_slots.rs` `wait_for_slot`,
  which says `queued behind` once.
- t19's p95 and max `waited_ms` of hook waits: how long people really wait.

- The roll-up label `wait for a slot, up to greengate.timeout:` names the
  setting without its value; pass the resolved length in (`up to 1h00s`)?
  (tui code review, t18.)

## Answer looks like

A short change to the heartbeat and its test, or a note that the waits are
short enough that it is not needed.
