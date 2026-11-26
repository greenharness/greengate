+++
type = "task"
mode = "afk"
status = "open"
due = "2026-11-27"
blocked_by = []
repos = ["amont"]
+++
# Count the journal 24 hours after the greengate 5.1.1 install

## Question

greengate 5.1.1 was installed on this machine at 2026-11-26 16:24 CET (brew; `greengate --version` printed
`greengate 5.1.1`; release #527, Release run 45847436802 conclusion success),
so the count is due from 2026-11-27 16:24 CET
([t18](t18-plan-hook-stall-narrow-fix.md)). After 24 hours, does
`waits.jsonl` show no hook deferral on a stalled line, and what is the hook
deferral rate beside the baseline? This is the first half of
[t08](t08-workday-measure.md)'s workday measure, taken early so a bad release
is found inside the day.

## Look at

- `/tmp/greenharness-slots-501/waits.jsonl`, rows from the install. Count wait
  rows only: `jq -cR 'fromjson? | select(has("waited_ms"))'`.
- [t17](t17-count-after-5-0-install.md)'s interim count and the plan
  `docs/plans/2026-11-26-hook-waits-no-stall-end.md`, `## Verification`.
- Rows without `holders` after the install (target 0), and the baked hook
  shims: a worktree install re-bakes a stale binary into the shared hooks.
- t17's step-2 table by `tool` and `outcome` (hook, `slot run`, relais): `slot
  run` timeouts and exit-75 stalls, relais deferred and
  `admission_wait_exhausted`. Hook waiters now stay in the shared line up to
  an hour, so a cost of 5.1.1 shows first in `slot run` and relais waits.
- Land's pre-stage rows (`prestage`) on their own: 57 of the 167 stalled rows
  were pre-stage, and a land on a stalled host now waits to the ceiling.

## Answer looks like

- Hook rows (`tool` greengate, not `slot run`) with `"end":"stalled"`: 0 is
  the pass condition of 5.1.1. Unqueued rows after a wait outside `now`: 0.
- Hook deferrals per 12 h beside 31.8, split by cause: `slotWait` (waited about
  60 s, `end` ceiling), `warnSlotWait` (about 600 s), `greengate.timeout`
  (about 3600 s). Expect the `slotWait` rows alone near 60 per 12 h; if the
  plain 2× rule (63.6 per 12 h) fires on those, the verdict is not a
  rollback of 5.1.1: it is the person's call on a like-for-like baseline or
  on `slotWait` (t08).
- p95 and max `waited_ms` of hook wait rows, and how many end at
  `greengate.timeout`. A hook wait at 3600 s is a finding for the person.
- Hook wait rows per 12 h against 227 (the post-5.0 rate): a clear drop with
  steady hook runs reads as waits aborted with Ctrl-C, which leave no row.
  If so, open a ticket for a `joined` journal row.
- A verdict: held, or what it reopens. A held verdict is carried into t08.
