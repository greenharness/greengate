+++
type = "task"
mode = "afk"
status = "open"
blocked_by = []
repos = ["amont", "relais"]
due = "2026-11-24"
+++
# The journal holds up a day after the 4.13 install

## Question

greengate/greenharness-slots 4.13.0 was installed 2026-11-23 04:59 CET and
relais v0.78.0 at 08:20 CET, with its coordinator restarted at 08:21
([t09](t09-journal-holders-and-hold-time.md)). After 24 hours,
does `waits.jsonl` meet the plan's `## Measure`? If not, roll back or file the
gap.

## Look at

- `/tmp/greenharness-slots-501/waits.jsonl`: greengate lines from 04:59
  CET, relais lines from 08:20 CET.
- `docs/host-slots.md` "The journal": the waits-only and pairing `jq` lines.
- The plan `docs/plans/2026-11-23-slot-journal-records-holders.md`,
  `## Measure`.

## Answer looks like

- After the install time, count the `admitted` lines by `tool` that lack
  `holders`. The target is 0: each one names a stale writer to update.
- Give the first `"tool":"relais"` line with `holders`, and its `released`
  line. If no relais run took a slot in the window, that is no failure. Say
  so, and carry this check to the next measure (t08) instead of rolling back.
- Count `released` lines whose `held_ms` is missing or not an integer
  (target 0). Give the share of `admitted` lines carrying `holders` that have
  no release: their pid is dead, or they are older than the longest hold.
- Lines per 12 hours against t08's baseline: 232 lines over 2026-11-21 18:00
  to 2026-11-22 06:00. Above 3× (696 per 12 hours), rotation becomes its own
  ticket.
- The plan's amended guard: the p95 of `waited_ms` over all `admitted` lines
  after the install (the split is by install time), against the same p95 over
  2026-11-21/22. Whole populations are compared because pre-4.13 lines carry
  no `holders`.
- Roll back when that p95 exceeds 2× the baseline, or on a panic in `Drop`.
  A panic shows on the holding command's stderr as `panicked at
  crates/greenharness-slots`: grep the greengate logs
  (`~/.claude/greenbelay/logs/`, the repos' `.git/greengate/logs/`) for it.
  Rolling back means reverting #510 in amont and releasing the next patch
  (4.13.1) through `tag-release`. For relais, it means reverting #545, then a
  v0.78.1 that pins greenharness-slots 4.12.
