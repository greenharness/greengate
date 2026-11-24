+++
type = "task"
mode = "afk"
status = "open"
blocked_by = []
repos = ["amont"]
+++
# The one-slot-wait test's 8 s bound breaks on a jammed host

## Question

`busy_defers::a_busy_push_waits_one_slot_wait_not_one_per_gate` failed in a
land's pre-stage on 2026-11-23 at load 50–150. At load 7 it passes 5 of 5 in
4.7 s against its 8 s wall-clock bound (`crates/greengate/tests/it/busy_defers.rs:564`).
The test means to prove that a push makes one slot wait, not one per gate. Can
it prove that by counting waits (the journal's `waits.jsonl` rows, or the
stderr wait lines) instead of by elapsed time?

## Look at

- `crates/greengate/tests/it/busy_defers.rs` near line 564.
- The `waits.jsonl` rows a push writes (`docs/host-slots.md`, "When a wait
  ends").

## Answer looks like

The test asserts the number of slot waits a push makes, with no wall-clock
bound, or with a bound loose enough that it cannot fail on load alone. It goes
red when each gate waits on its own (prove it by breaking the one-wait handoff),
and it passes at load 40 or more.
