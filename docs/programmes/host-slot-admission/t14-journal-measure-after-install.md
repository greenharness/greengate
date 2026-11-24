+++
type = "task"
mode = "afk"
status = "closed"
blocked_by = []
repos = ["amont", "relais"]
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

## Resolution

Measured 2026-11-24 10:35 CET, 29.6 hours after the 4.13.0 install (04:59
CET 2026-11-23); relais rows count from its 08:20 install. The journal meets
the plan's `## Measure`. No rollback.

- Admitted rows without `holders` after install: 1 genuine, 3 in flight.
  Three greengate rows (pids 45448 twice, 57571) waited from before the
  install, so an old binary wrote them. The one gap is `greengate clippy`,
  pid 2230, 2026-11-23 15:40 UTC, waited 21 min, started after the install: a
  stale writer. `node_modules/.bin/greengate` is 4.11.0 in four greenrack
  worktrees (`-f05`, `-f05-before`, `-f110-base`, `-first-touch`), the
  version split in the memory "worktree install re-bakes shared hooks". relais: 0 rows
  without `holders` after 08:20 CET (5 admitted, 5 released).
- First relais row with `holders`: pid 17293, 2026-11-23 10:36:38 UTC
  (`holders` 1, `holder_jobs` 2, waited 820685 ms), released 13 s later,
  `held_ms` 13455.
- `released` rows: 210 (greengate 149, `slot run` 56, relais 5); 0 with a
  missing or non-integer `held_ms`.
- Admitted rows with `holders` and no release row (matched by pid): 6 of 217,
  2.8 %.
- Volume: 717 rows in 29.6 h, 291 per 12 h, 1.25× the 232 baseline against a
  696 limit. Rotation needs no ticket.
- p95 of `waited_ms` over all admitted rows: 3,121,563 ms after install
  (n 224) against 2,543,454 ms over 2026-11-21/22 (n 389): 1.23×, under the
  2× guard.
- Panics: none in `Drop`. The one grep hit for `panicked at
  crates/greenharness-slots` is a cargo-test log from 2026-11-24 07:15 CET
  (two tests at `lib.rs:1798`, "expected to wait"), written while t11's
  admission rule was under test, not a holding command's stderr.

Open: the four greenrack worktrees still run greengate 4.11.0 from
`node_modules`; a worktree install re-bakes it. Their next `pnpm install` or
`/usr/local/bin/greengate install` fixes it.
