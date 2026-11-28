+++
type = "task"
mode = "afk"
status = "closed"
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

## Resolution

Yes: say where it ends and how to skip it. Hook waits are long. From the 5.1.1
install (2026-11-26 16:24 CET) to the journal's last row (2026-11-27 14:23 CET,
about 22 h), hook rows only (`tool = "greengate"`, `released` lines left out):
35 waits, p50 106 s, p95 896 s, max 1104 s; 14 of them over 300 s. The 20 that
were admitted after a wait had p50 367 s and max 1104 s, in a line that showed
no end. t19 (due 2026-11-27 16:24) had not run; this is not its verdict.

The queued heartbeat now reads `queued 1m00s for a host slot
(greengate.hostSlots 2), defers to CI in at most 59m00s (greengate.timeout);
skip the queue: rerun with GREENGATE_ON_BUSY=now`. The end is what is left of
`greengate.timeout` (3600 s when it is 0, one shared `wait_ceiling_secs`), an
upper bound: an `onBusy defer` check ends at 60 s and a `warn` check at 600 s.
The skip names the variable only, because `on_push` cannot name the command run
(false in a land rehearsal, true under `greengate run --raw`); inside a land
rehearsal, which sets `onBusy wait` itself, the skip clause is left out. The
roll-up label now carries the resolved length: `wait for a slot, up to 1h00m
(greengate.timeout):`.

Not done: each check's own bound (11 of the 35 waits were 60 s `slotWait` ones
and still read "at most 59m00s") needs the per-thread wait plan in the live
row. Plan: `docs/plans/2026-11-27-queued-heartbeat-names-ceiling.md`.
