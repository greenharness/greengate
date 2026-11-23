+++
type = "task"
mode = "afk"
status = "closed"
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

## Resolution

Built 2026-11-23 (plan `docs/plans/2026-11-23-slot-journal-records-holders.md`),
released in greenharness-slots 4.13.0.

- Every wait line carries `holders` and `holder_jobs`, read inside the crate
  from the live holder records, so every writer gets them with no caller
  change. An `admitted`/`handed` line counts its own lease. `null` means a
  record could not be read; a line without the fields is an older writer.
- Holder records carry a private `lease` id. `holder_jobs` dedupes by lease,
  so sibling leases count apart and a lease seen mid-rename counts once.
  `Holder` and `greengate slots --json` are unchanged.
- Every dropped lease appends one `"outcome":"released"` line with
  `granted_ms` and a monotonic `held_ms`. This is narrower than "each lease":
  a holder that exits without unwinding (a kill, `land` on a signal) leaves
  none, so `held_ms` is censored toward short holds. t08's notes say how to
  read this.
- Pilot (two overlapping `slot run`s, 2 slots): the second `admitted` line
  had `holders 2`, `holder_jobs 6`. The release lines had `held_ms` 2029 and
  5024 for `sleep 2` and `sleep 5`.
- relais writes the same fields once its lock moves from 4.0.0 to 4.13.0
  (Phase 4 of the plan).
