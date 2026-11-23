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

greengate/greenharness-slots 4.13.0 and relais v0.78.0 were installed on
2026-11-23 ([t09](t09-journal-holders-and-hold-time.md)). After 24 hours,
does `waits.jsonl` meet the plan's `## Measure`? If not, roll back or file the
gap.

## Look at

- `/tmp/greenharness-slots-501/waits.jsonl`, from 2026-11-23 08:00.
- `docs/host-slots.md` "The journal": the waits-only and pairing `jq` lines.
- The plan `docs/plans/2026-11-23-slot-journal-records-holders.md`,
  `## Measure`.

## Answer looks like

- After the install time, count the `admitted` lines by `tool` that lack
  `holders`. The target is 0: each one names a stale writer to update. Also
  give the first `"tool":"relais"` line with `holders`, and its `released`
  line.
- Count every `released` line with `held_ms` ≥ 0. Give the share of
  `admitted` lines carrying `holders` that have no release: their pid is
  dead, or they are older than the longest hold.
- Lines per 12 hours against the 232 baseline. Above 3× it, rotation
  becomes its own ticket.
- The p95 of `waited_ms` on all `admitted` lines before and after the
  install. The pre-4.13 lines have no `holders`, so the guard compares
  whole populations. Roll back at 2× the baseline, or on any panic in
  `Drop`.
