+++
type = "task"
mode = "afk"
status = "open"
due = "2026-11-25"
blocked_by = []
repos = ["amont", "relais"]
+++
# Count the journal 24 hours after the greengate 5.0 install

## Question

greengate 5.0.0 was installed on this machine on 2026-11-24 at about 12:49
CET ([t11](t11-build-admission-and-waits.md) Phase 4). After 24 hours, does
`waits.jsonl` meet the plan's `## Verification` count? This is the first half
of [t08](t08-workday-measure.md)'s workday measure, taken early so a bad
release is rolled back inside the day.

## Look at

- `/tmp/greenharness-slots-501/waits.jsonl`, rows from 2026-11-24 12:49 CET.
  Count waits only, as [t14](t14-journal-measure-after-install.md) and
  t08 say: `jq -cR 'fromjson? | select(has("waited_ms"))'`.
- The plan `docs/plans/2026-11-23-admission-counts-holders-stalled-waits.md`,
  `## Verification`, last bullet, for the baseline: hook unqueued 34, hook
  defer 103, `slot run` timeouts 50, relais deferred 21 (since 2026-11-21
  18:00).
- The first step below, which this count depends on.

## Steps

1. Refresh the stale shims first, or the count mixes writers. The four
   greenrack worktrees (`-f05`, `-f05-before`, `-f110-base`, `-first-touch`)
   run greengate 4.11.0 from `node_modules`, and a worktree install re-bakes
   it into the shared hooks. Run `/usr/local/bin/greengate install` in the
   live greenrack checkout, then check `greengate --version` through each
   baked hook shim reads 5.0.0 (memory "worktree install re-bakes shared
   hooks"). Remove the four worktrees if they are swept.
2. Count by `tool`: rows without `holders` after the install (target 0 from a
   5.0 writer; each one names a stale writer), `outcome` by kind, rows with
   `"end":"stalled"` and `"end":"ceiling"`, rows with outcome `now`.
3. Unqueued rows after a wait, outside `now` (target 0), and runs blocked
   `admission_wait_exhausted` after a grant (target 0; relais counts from its
   own Phase 5 install, so report it only when that has happened).
4. The hook deferral rate against 103 per 12 hours scaled to the window.

## Answer looks like

The counts of steps 2–4 beside the baseline, and a verdict: held, or roll back.
Roll back when the hook deferral rate passes 2× the baseline: revert the
crate change in amont and release the next patch through `tag-release`.
A held verdict is carried into t08.
