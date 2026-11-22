+++
type = "research"
mode = "afk"
status = "open"
blocked_by = []
repos = ["amont"]
+++
# Admission stops counting the holders' own load

## Question

Auto admission opens a second or third slot only while `load1 < cores` (6
physical cores here). One heavy holder, such as greengate's integration suite at
`jobs 2`, keeps load above 6 by itself, so the cap of 3 behaves like 1. Which
rule keeps the load test as a defence against outside load, but does not count
the load the current holders are expected to make? Candidates: compare against
`cores + Σ holders' jobs`, subtract a measured per-holder load, or keep `load1 <
cores` only for load the queue does not own.

## Look at

- `crates/greenharness-slots/src/lib.rs`: `Waiter::poll_with` (the `below`
  test), `Probe`, `RAMP`, `parallelism`, `read_holders` (holders carry `jobs`).
- `waits.jsonl` (`/tmp/greenharness-slots-501/`): rows carry `load1`, `jobs`,
  `cap`, `cores`; estimate the load one holder adds from rows where one holder
  ran alone.
- The 2026-11-10 plan's reasoning for the physical-core cap (memory
  `slots_physical_cores_plan_2026_11_10`); ADR-0013 and ADR-0014 through aval.

## Answer looks like

One admission rule as a formula, with the numbers from `waits.jsonl` that
justify it, a worked example on 2026-11-22's state (1 holder, load 12.3, 5
waiting → admitted or not), and how it behaves at last night's peak (load 300+
from outside: must still hold back). Plus the unit test seam in `poll_with` that
goes red on today's rule.
