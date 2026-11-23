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
- From greenharness-slots 4.13 ([t09](t09-journal-holders-and-hold-time.md),
  `docs/host-slots.md` "The journal"):
  - Count acquisitions on wait lines only:
    `jq -cR 'fromjson? | select(has("waited_ms"))'`. Release lines have no
    `waited_ms`.
  - An `admitted` or `handed` line's `holders` and `holder_jobs` include its
    own lease: subtract its `weight` and `jobs` for the k = 4 check.
  - A line without `holders` comes from an older writer (relais before its
    4.13 bump, a stale baked hook binary). Its occupancy is unknown, not 0;
    leave it out of occupancy, pairing and the k = 4 check. Count such lines
    by `tool` after each install; the target is 0.
  - A release pairs with the first `admitted` line of the same `pid` and
    `tool` at or after its `granted_ms`. A process holding several leases at
    once can pair ambiguously, and relais's release `session` is its run id,
    not the env session its wait lines carry.
  - An `admitted` line with `holders` and no release is an interrupted
    holder (killed, or `land` stopped by a signal), unless its `pid` is
    still alive at the window's end. `held_ms` is therefore censored toward
    short holds: report the unpaired count beside every hold-time figure.
  - Order lines by `at_ms`, not by line: a release is appended after its
    slot is freed.

## Answer looks like

For the workday: admissions per hour, the share of time with 3 slots held while
jobs waited, timeouts that happened while the line moved (target 0), unqueued
heavy runs (target 0), and every reorder with its reason. Each compared with the
baseline, and a verdict: destination reached, or the ticket it reopens.
