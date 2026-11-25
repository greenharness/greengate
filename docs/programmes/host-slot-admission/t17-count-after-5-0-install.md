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

## Interim count (2026-11-25 02:12 CET, 13.4 h after install)

Install: `/usr/local/bin/amont` → Cellar greengate 5.0.0, 2026-11-24 12:49:28
CET. Counted on wait rows only (`has("waited_ms")`).

Step 1, shims. The four named greenrack worktrees: `-f05`, `-f05-before`
and `-f110-base` are gone; `-first-touch` holds a live session and still
carries 4.11.0, so it was left alone. `greenrack-wt-f13-base` re-baked
greenrack's shared hooks to its 4.11.0 at 02:05; `duro-app`'s shims pointed
at a removed `duro-app-wt-scope` 4.11.0 path (they fell through to PATH,
5.0). Both re-baked with `/usr/local/bin/greengate install`; every
repository under `~/Developer/Perso` now bakes the 5.0 binary, its `amont`
alias, or a placeholder that falls back to PATH.

Step 2, after install, by `tool`:

| tool | outcome | end | rows |
| --- | --- | --- | --- |
| greengate (hook) | admitted | — | 73 |
| greengate (hook) | handed | — | 22 |
| greengate (hook) | deferred | ceiling | 83 |
| greengate (hook) | deferred | stalled | 75 |
| greengate slot run | admitted | — | 39 |
| greengate slot run | now | — | 8 |
| greengate slot run | timeout | stalled | 43 |
| greengate slot run | timeout | ceiling | 6 |
| relais | admitted | — | 9 |
| relais | deferred | — | 11 |

Rows without `holders`: 0 (target met).

Step 3. Unqueued rows after a wait, outside `now`: 0 (target met).
`admission_wait_exhausted` after a grant: not counted yet. relais's
Phase 5 (the `greenharness-slots = "5"` bump) has not shipped (relais
0.82.0 installed).

Step 4, hook deferrals. The ticket's "103 per 12 hours" was wrong. The
plan's 103 runs from 2026-11-21 18:00 to the 103rd deferral at 2026-11-23
08:54 CET (38.9 h), so the baseline is 31.8 per 12 h. A recount over the
whole pre-install window (66.8 h, 193 deferrals) gives 34.6 per 12 h.

| | baseline | after install (13.4 h) | ratio |
| --- | --- | --- | --- |
| hook deferrals per 12 h | 31.8 | 141.8 (158 rows) | 4.5× |
| deferrals + former unqueued per 12 h | 42.3 (103 + 34) | 141.8 | 3.4× |
| `slot run` timeouts | 50 in ~39 h | 49 in 13.4 h | — |
| relais deferred | 21 in ~39 h | 11 in 13.4 h | — |

The verdict is locked before 24 h. Twice the plain baseline over 24 h is
127 deferrals, and 158 are already counted, so at 12:49 the rate is at
least 2.5× whatever the night does.

Context, which does not change the rule. Hook wait rows doubled (112 → 227
per 12 h), and the median `load1` rose from 11.0 to 17.1. One session
(`f138271b`) wrote 50 of the deferrals. Ceiling deferrals alone, a path
the change did not add, run at 74.5 per 12 h. By name: cargo-test 67,
clippy 36, adr-lint-cli-probe 29, run-tests-js 16, plugin-test 10.

Verdict: **roll back**. The hook deferral rate passes 2× the baseline under
every reading.
