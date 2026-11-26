+++
type = "task"
mode = "afk"
status = "open"
due = "2026-12-03"
blocked_by = []
repos = ["amont"]
+++
# Count queue moves seven days after the greengate 5.2.0 install

## Question

greengate 5.2.0 (`greengate slot promote`, [t13](t13-build-reorder.md); #530,
release #531, Release run 45889748609 success) was installed on this machine
at 2026-11-26 21:21 CET (brew; `greengate --version` printed
`greengate 5.2.0`). Over the seven days that follow, how many `promoted` lines
does each session write, how many `promote_refused` lines by `limit`, and is
p95 `waited_ms` within 1.5× of t14's baseline (the plan's rollback trigger)?
Zero moves in 7 days reads as agents not finding the command. Does t06's limit (3 moves per
session per day, never past a 20-minute waiter) hold, or does it need a new
value?

## Look at

- `/tmp/greenharness-slots-501/waits.jsonl`: `promoted` and `promote_refused`
  rows from the install; wait rows (`has("waited_ms")`) for p95.
- [t14](t14-journal-measure-after-install.md)'s baseline and the plan
  `docs/plans/2026-11-26-agent-moves-own-job-ahead.md`, its `## Measure`.

## Answer looks like

Counts by session and by refusal limit, the p95 beside t14's, and a verdict:
the limits stay, or a named new value with its reason. Above 1.5× of t14's
p95, promotion is turned off: through [t23](t23-promote-off-switch.md)'s switch
when it has shipped, else by reinstalling greengate 5.1.1.
