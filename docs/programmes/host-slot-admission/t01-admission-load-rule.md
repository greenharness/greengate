+++
type = "research"
mode = "afk"
status = "closed"
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

## Resolution

Research run 2026-11-22 (Explore agent over `lib.rs` and `waits.jsonl`).

**Rule.** For Auto sizing, when a slot is already held, admit when `position == 0`,
the slot is free, the ramp has elapsed, and `load1 < cores + k × Σjobs(holders)`,
with `k = 4`. Put the other way round, `load1 − 4·Σjobs < cores`. The floor
(admit when nothing is held) and the cap both stay. `Σjobs` sums `Holder.jobs`
from `read_holders`. A holder with no readable record counts as
`parallelism(1, cores, cap)`, which is 2 here. The load-reason branch must use
the same threshold.

**Numbers.** The journal holds 1,893 rows, all at `cap 3`, `cores 6`, `jobs 2`,
apart from 13 rows at `jobs 4`. Floor admissions, those that waited under 1 s,
saw `load1` with median 5.8 and p25 4.1. One holder at `jobs 2` measured
12.3 on 2026-11-22. That is about 8 above a baseline of about 4, so about 4 per
job. `k = 1` (limit 8) and `k = 3` (limit 12) would both still refuse 12.3.
Under today's rule, 41 timeouts and 42 deferrals happened at `load1 < 12`.

**Worked examples.** One holder at load 12.3 gives a limit of 14, so the job is
admitted (today it is refused). Outside load of 300 to 635, with at most 2
holders, gives a limit of 22, so the job is still held back.

**Test seam.** `Waiter::poll_with` with an injected `Probe { load1: 12.3,
cores: 6, now_ms past RAMP }`, one locked slot, and `holders/slot-0.who` with
`jobs: 2` (the same way as the sibling-probe test). It expects `Granted`;
today's rule returns `Waiting(Load)`. A second case at `load1 300` expects
`Waiting(Load)`.

**Risks, accepted.** `k` rests on one observation, and the journal records
neither a holder count nor hold time, so the rule cannot be checked on a
workday yet. That is now [t09](t09-journal-holders-and-hold-time.md). Floor
admissions still pass at any load (311 of them were at `load1 ≥ 18`).
Memory pressure is still fog.
