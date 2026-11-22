+++
type = "task"
mode = "afk"
status = "open"
blocked_by = []
repos = ["amont", "relais"]
+++
# The journal records holders and hold time

## Question

`waits.jsonl` has one row per acquisition, with the wait but not how many slots
were held at that moment, nor how long the slot was then held. t01's `k = 4`
rests on one observation, and t07 could not measure how long a setup holds a
slot. Add `holders` (the count and the `Σjobs`) to every row, and a `released`
row (or `held_ms` on release) for every lease, written the same way by both
tools.

## Look at

- `crates/greenharness-slots/src/lib.rs`: the journal writer (the
  `"{\"at_ms\":…}"` format line), `Lease` and its `Drop`, `read_holders`.
- relais's use of the crate (`verify/heavy.rs`): whether it writes through the
  same function.

## Answer looks like

A merged change, released, after which a new `waits.jsonl` row carries
`holders` and `holder_jobs`, and each lease leaves one release row with its
`held_ms`. Older readers ignore the new fields; the slot crate's own tests show
both shapes. This lands before the builds of t01, t02 and t07, so the t08
measure can compare.
