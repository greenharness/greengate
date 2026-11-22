+++
type = "task"
mode = "afk"
status = "open"
blocked_by = ["t11-build-admission-and-waits", "t12-build-relais-setup-lease", "t13-build-reorder"]
repos = ["amont"]
+++
# Measure one workday of the queue after the builds

## Question

Once the builds these tickets lead to are released and installed, does one
workday of `waits.jsonl` meet the destination? The baseline is
2026-11-21 18:00 to 2026-11-22 06:00: 232 rows, 103 admitted, 38 timeout, 29
unqueued, and 1 of 3 slots held while 5 jobs waited at load 12.

## Look at

- `/tmp/greenharness-slots-501/waits.jsonl` (rows carry `at_ms`, `outcome`,
  `load1`, `waited_ms`, `cap`, `jobs`, `session`).
- `greengate slots --json`, sampled during the day.

## Answer looks like

For the workday: admissions per hour, the share of time with 3 slots held while
jobs waited, timeouts that happened while the line moved (target 0), unqueued
heavy runs (target 0), and every reorder with its reason. Each compared with the
baseline, and a verdict: destination reached, or the ticket it reopens.
