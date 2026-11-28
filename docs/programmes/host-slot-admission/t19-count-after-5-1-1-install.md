+++
type = "task"
mode = "afk"
status = "closed"
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

## Resolution

Held: no rollback, carried into [t08](t08-workday-measure.md).

**Window.** greengate 5.1.1 ran alone for only 5 hours (16:24 to 21:21 CET on
2026-11-26, when 5.2.0 was installed, see [t22](t22-count-after-5-2-0-install.md)).
The journal rotated before the count: its first row is 2026-11-27 11:22 CET.
So the count covers every row on 2026-11-28 13:41 CET, 26.3 h from 11:22 CET
on 11-27 to 13:38 CET on 11-28, under 5.2.0 and then 5.4.0. Both keep 5.1.1's
hook path (t18): a hook check has no stall window. Its rows carry no version
field.

Wait rows (`has("waited_ms")`): 390.

- **Pass conditions, all met.** Hook rows (`tool` greengate) with
  `"end":"stalled"`: 0 of 249. Rows without `holders`: 0. Rows with
  `queued` false outside `now`: 0.
- **Hook deferrals.** 82, or 37.5 per 12 h beside 31.8 (1.18×, under the 2×
  trigger of 63.6). By cause: about 60 s (`slotWait`): 76, or 34.7 per 12 h,
  half the expected rate of about 60. About 600 s (`warnSlotWait`): 6. About
  3600 s (`greengate.timeout`): 0.
- **Hook waits.** p95 1101 s, max 3537 s. 0 ended at `greengate.timeout`. All
  20 waits over 900 s were admitted. The two near the hour (3504 s, 3537 s, at
  00:11 CET on 11-28) fell in the load-410 episode, caused by another
  session's unqueued test loop (see the map's Handoff lessons). That was load
  outside the queue, not a regression of 5.1.1.
- **Hook wait rows per 12 h.** 113.8 beside 227 after 5.0. That is a clear
  drop. The journal alone cannot tell whether hook runs held steady, since a
  wait aborted with Ctrl-C leaves no row. So whether that drop means aborted
  waits is t08's question, and a `joined` journal row is filed as a
  follow-up.
- **`slot run`.** 132 rows: 77 admitted, 9 `now`, 9 timeout at the ceiling,
  37 timeout on a stall. The stall exits are the cost t18 predicted, and
  [t21](t21-slot-run-waits-past-a-stall.md)'s subject.
- **relais.** 9 rows: 4 admitted, 5 deferred on a stall.
- **Pre-stage rows.** None are named `prestage` in this window.

Commands (jq over `/tmp/greenharness-slots-501/waits.jsonl`, in the driver
session of 2026-11-28): wait rows grouped by `tool`, `outcome` and `end`;
deferrals bucketed by `waited_ms` (< 120 s, < 900 s, else); hook waits over
900 s listed with `load1`.
